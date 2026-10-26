"""WritingDeck protocol 1, exact-version optional offline prose adapters.

Only a bounded masked projection arrives on stdin, never a path to a novel.
No cwd config discovery, downloads, AI-detector/authorship rules or score.
Nabokov uses a trained spaCy pipeline and requires separate explicit permission.
Proselint is non-model. Original text/replacements remain the Rust editor's job.
"""
import contextlib
import hashlib
import importlib.metadata
import io
import json
import os
import resource
import socket
import sys
import time

SCHEMA = 1
MAX_BYTES = 65536
MAX_REQUEST = 524288
MAX_FINDINGS = 256
VERSIONS = {'proselint': '0.16.0', 'nabokov': '26.7.10', 'spacy': '3.8.16', 'en_core_web_sm': '3.8.0'}
ENGINE_VERSIONS = {
    'proselint': '0.16.0-dbed789caae6/adapter1',
    'nabokov': '26.7.10-e6f98ce64b52/spacy3.8.16/en3.8.0/adapter1',
}
PACKAGE_DIGESTS = {'nabokov': {'module': 'nabokov', 'tree_sha256': 'dfe09a3f8caba9b57e345ed19ce18b60534987250bd101f146b209dd6edc2e59', 'files': 58, 'version': '26.7.10'}, 'proselint': {'module': 'proselint', 'tree_sha256': '39aab06834f8d60c7b8fd46e33a6fb0db583f9f7a725dff3fd9c4a55341a6aff', 'files': 110, 'version': '0.16.0'}, 'en_core_web_sm': {'module': 'en_core_web_sm', 'tree_sha256': 'cb57d232ab2a6e466a19ce524bbd0a04d1bee22f92eeb8b99d754fa3b303e18c', 'files': 28, 'version': '3.8.0'}}
NABOKOV_CODES = frozenset(('NB201', 'NB202', 'NB203', 'NB301', 'NB302', 'NB303', 'NB304', 'NB305', 'NB401'))
PROSELINT_GROUPS = ('cliches', 'hedging', 'lexical_illusions', 'malapropisms', 'misc', 'mixed_metaphors', 'nonwords', 'redundancy', 'spelling.ve_of', 'uncomparables', 'weasel_words')


def deny_network(*args, **kwargs):
    raise RuntimeError('network unavailable in offline prose worker')


def deny_kernel_network_and_children():
    """Fail-closed Linux amd64 syscall boundary, before third-party imports.

    Socket monkeypatches alone do not constrain C extensions. Deny all socket
    operations, io_uring (including socket ops), child/thread creation, exec and
    cross-process inspection even through libc. Reject x32/foreign architectures.
    This is NOT a complete filesystem/malicious-code sandbox or security audit.
    The Rust parent passes pipes only, no inherited network descriptors.
    """
    import ctypes
    if not sys.platform.startswith('linux') or os.uname().machine != 'x86_64':
        raise RuntimeError('pinned offline worker requires Linux x86_64 syscall isolation')

    class Filter(ctypes.Structure):
        _fields_ = [('code', ctypes.c_ushort), ('jt', ctypes.c_ubyte), ('jf', ctypes.c_ubyte), ('k', ctypes.c_uint32)]

    class Program(ctypes.Structure):
        _fields_ = [('length', ctypes.c_ushort), ('filters', ctypes.POINTER(Filter))]

    errno = 0x00050000 | 1  # SECCOMP_RET_ERRNO | EPERM
    instructions = [
        (0x20, 0, 0, 4),             # seccomp_data.arch
        (0x15, 1, 0, 0xC000003E),    # AUDIT_ARCH_X86_64
        (0x06, 0, 0, 0x80000000),    # KILL_PROCESS on foreign ABI
        (0x20, 0, 0, 0),             # seccomp_data.nr
        (0x35, 0, 1, 0x40000000),    # reject x32/high-bit syscall bypass
        (0x06, 0, 0, errno),
    ]
    denied = tuple(range(41, 56)) + (56, 57, 58, 59, 101, 288, 299, 304, 307, 310, 311, 321, 322, 323, 425, 426, 427, 435)
    for number in denied:
        instructions.extend(((0x15, 0, 1, number), (0x06, 0, 0, errno)))
    instructions.append((0x06, 0, 0, 0x7FFF0000))  # otherwise ALLOW
    filters = (Filter * len(instructions))(*(Filter(*i) for i in instructions))
    program = Program(len(instructions), filters)
    libc = ctypes.CDLL(None)
    if libc.prctl(38, 1, 0, 0, 0) != 0 or libc.prctl(22, 2, ctypes.byref(program), 0, 0) != 0:
        raise RuntimeError('kernel offline-worker syscall boundary unavailable')


def configure_process():
    # Linux parent-death binding: no model process may survive a normal/crashed
    # Grimoire owner. Parent PID is numeric metadata, never source or credentials.
    if sys.platform.startswith('linux'):
        import ctypes
        parent = int(os.environ.get('WRITINGDECK_PARENT_PID', os.getppid()))
        if os.getppid() != parent or ctypes.CDLL(None).prctl(1, 9, 0, 0, 0) != 0 or os.getppid() != parent:
            raise RuntimeError('offline worker parent binding unavailable')
    deny_kernel_network_and_children()
    # Friendlier Python errors in addition to the kernel denial, not a complete
    # filesystem sandbox or a replacement for independent security review.
    socket.socket.connect = deny_network
    socket.socket.connect_ex = deny_network
    socket.create_connection = deny_network
    socket.getaddrinfo = deny_network
    # Avoid BLAS thread pools on low-power systems; the parent also sets these
    # before interpreter startup. No persistent Python/model process is kept.
    for key in ('OPENBLAS_NUM_THREADS', 'OMP_NUM_THREADS', 'MKL_NUM_THREADS', 'NUMEXPR_NUM_THREADS'):
        os.environ[key] = '1'
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    resource.setrlimit(resource.RLIMIT_NPROC, (0, 0))
    resource.setrlimit(resource.RLIMIT_CPU, (30, 32))


def checked_version(package):
    if importlib.metadata.version(package) != VERSIONS[package]:
        raise RuntimeError('unsupported dependency version')
    if package in PACKAGE_DIGESTS:
        record = PACKAGE_DIGESTS[package]
        distribution = importlib.metadata.distribution(package)
        items = []
        for item in distribution.files or ():
            if item.parts[0] != record['module'] or item.suffix == '.pyc' or '__pycache__' in item.parts:
                continue
            items.append((item.as_posix(), hashlib.sha256(distribution.locate_file(item).read_bytes()).hexdigest()))
        digest = hashlib.sha256()
        for name, value in sorted(items):
            digest.update(name.encode() + b'\0' + value.encode() + b'\n')
        if len(items) != record['files'] or digest.hexdigest() != record['tree_sha256']:
            raise RuntimeError('pinned package/data integrity mismatch')


def literal_alternatives(suggestion, applicability, expected):
    # Upstream prose directions and comma-separated choices are not literals.
    if applicability != 'replace' or not isinstance(suggestion, str):
        return []
    if len(suggestion.encode('utf-8')) > 1024 or any(c in suggestion for c in ('\n', '\r', ',', '/', '|', ' or ')):
        return []
    if any(ord(c) < 32 for c in suggestion):
        return []
    if expected[:1].isupper() and suggestion:
        suggestion = suggestion[:1].upper() + suggestion[1:]
    return [suggestion]


def proselint_findings(text):
    checked_version('proselint')
    from proselint.checks import __register__
    from proselint.registry import CheckRegistry
    from proselint.tools import LintFile
    registry = CheckRegistry()
    registry.register_many(__register__)
    if not registry.checks:
        raise RuntimeError('empty check registry is not a clean report')

    class FictionFile(LintFile):
        def is_quoted_pos(self, position):
            # A novel's dialogue is prose. Rust applies style-specific relaxation
            # by original-source context; usage is not suppressed wholesale.
            return False

    config = {'max_errors': 4096, 'checks': {group: True for group in PROSELINT_GROUPS}, 'per_file_checks': {}}
    source = FictionFile('<masked-memory>', text)
    for lint in source.lint(config):
        result = lint[0]
        # This exact API pads with ONE leading and one trailing newline. Its
        # CheckResult spans are Python characters in that padded string, not
        # UTF-8 bytes and not the old 0.14 CLI schema.
        start, end = result.span[0] - 1, result.span[1] - 1
        if not (0 <= start < end <= len(text)):
            raise RuntimeError('invalid upstream span')
        expected = text[start:end]
        rule = result.check_path
        usage = rule.startswith(('spelling.ve_of', 'malapropisms.', 'nonwords.', 'misc.', 'lexical_illusions.'))
        yield {'rule': rule, 'category': 'usage' if usage else 'style', 'start': start, 'end': end, 'expected': expected,
               'message': result.message, 'alternatives': literal_alternatives(result.replacements, 'replace', expected)}


def nabokov_findings(text):
    for package in ('nabokov', 'spacy', 'en_core_web_sm'):
        checked_version(package)
    from nabokov import analyzer
    from nabokov.config import Config
    from nabokov.source import SourceFile
    from nabokov.checks import ALL_RULES
    from nabokov.checks.base import CheckContext
    # Never call Engine.nlp's auto-download default. Explicitly load an already
    # installed model with downloads disabled, then invoke only audited rules.
    nlp = analyzer.load_nlp(auto_download=False)
    config = Config(select=tuple(sorted(NABOKOV_CODES)), target='NORMAL', style=None, max_grade=None)
    source = SourceFile.from_text(text, '<masked-memory>')
    context = CheckContext(doc=nlp(text), source=source, config=config, nlp=nlp)
    # We deliberately do not call Engine.analyze: its _drop_quoted pass discards
    # fictional dialogue. No monkeypatching and no NB5xx/NB7xx detector/profile
    # methods, objective grade/score, implicit budgets or file-config discovery.
    for rule in ALL_RULES:
        if not set(rule.codes).intersection(NABOKOV_CODES):
            continue
        for result in rule.check(context):
            if result.code not in NABOKOV_CODES:
                continue
            start = source.offset(result.line, result.col)
            end = source.offset(result.end_line, result.end_col)
            if not (0 <= start < end <= len(text)):
                raise RuntimeError('invalid upstream span')
            expected = text[start:end]
            if result.text and result.text != expected:
                raise RuntimeError('upstream text does not match character span')
            alternatives = literal_alternatives(result.suggestion, result.applicability, expected)
            message = result.message
            if result.suggestion is not None and result.applicability == 'rewrite':
                message += ' Direction only, not a literal replacement: ' + result.suggestion
            yield {'rule': result.code, 'category': 'readability' if result.code.startswith('NB2') else 'style',
                   'start': start, 'end': end, 'expected': expected, 'message': message, 'alternatives': alternatives}


def run(request):
    if set(request) != {'schema', 'engine', 'request_id', 'sha256', 'text'}:
        raise ValueError('unsupported request fields')
    text = request['text']
    engine = request['engine']
    if request['schema'] != SCHEMA or engine not in ENGINE_VERSIONS or type(request['request_id']) is not int or not (0 <= request['request_id'] <= 2**64-1):
        raise ValueError('invalid request schema, engine or generation')
    if not isinstance(text, str) or not text or len(text.encode('utf-8')) > MAX_BYTES:
        raise ValueError('empty/oversized input is not a clean report')
    digest = hashlib.sha256(text.encode('utf-8')).hexdigest()
    if request['sha256'] != digest:
        raise ValueError('input digest mismatch')
    started = time.monotonic()
    out = {'schema': SCHEMA, 'engine': engine, 'version': ENGINE_VERSIONS[engine], 'request_id': request['request_id'],
           'sha256': digest, 'status': 'complete', 'findings': [], 'omitted': 0, 'elapsed_ms': 0, 'peak_rss_kib': 0}
    try:
        iterator = proselint_findings(text) if engine == 'proselint' else nabokov_findings(text)
        for finding in iterator:
            if len(out['findings']) < MAX_FINDINGS:
                out['findings'].append(finding)
            else:
                out['omitted'] += 1
    except importlib.metadata.PackageNotFoundError:
        out.update(status='unavailable', findings=[], reason='required pinned dependency/model is not installed')
    except Exception as error:
        # No exception message/traceback: third-party exceptions may quote prose.
        out.update(status='failed', findings=[], reason='pinned offline engine failed: ' + type(error).__name__)
    out['elapsed_ms'] = round((time.monotonic()-started)*1000)
    out['peak_rss_kib'] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return out


def main():
    configure_process()
    try:
        payload = sys.stdin.buffer.read(MAX_REQUEST + 1)
        if len(payload) > MAX_REQUEST:
            raise ValueError('oversized protocol input')
        request = json.loads(payload)
        # Never forward unsolicited third-party stdout, especially not text.
        with contextlib.redirect_stdout(io.StringIO()):
            response = run(request)
    except Exception as error:
        response = {'schema': SCHEMA, 'status': 'failed', 'reason': 'invalid offline request: ' + type(error).__name__}
    sys.stdout.write(json.dumps(response, ensure_ascii=True, allow_nan=False, separators=(',', ':')) + '\n')


if __name__ == '__main__':
    main()
