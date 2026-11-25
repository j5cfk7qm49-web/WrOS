{ pkgs, lib, src, hayroBench }:
let
  bench = src + "/tools/brain-pdf-ocr-bench.py";
  recipe = src + "/tools/hp-measure";
  pdfium = pkgs.pdfium-binaries;
  poppler = pkgs.poppler-utils;
  tesseract = pkgs.tesseract5.override { enableLanguages = [ "chi_sim" "chi_sim_vert" ]; };
  jbig2 = pkgs.jbig2enc or null;
  python = pkgs.python3.withPackages (ps: [ ps.pillow ]);

  # Keep the three upstream English models separate from the executable. Each fetch is a
  # fixed-output input, so corpus generation and the installed benchmark need no network.
  english = tier: repo: sha256: pkgs.fetchurl {
    name = "eng-${tier}.traineddata";
    url = "https://raw.githubusercontent.com/tesseract-ocr/${repo}/4.1.0/eng.traineddata";
    inherit sha256;
  };
  fast = english "fast" "tessdata_fast" "7d4322bd2a7749724879683fc3912cb542f19906c83bcc1a52132556427170b2";
  standard = english "standard" "tessdata" "daa0c97d651c19fba3b25e81317cd697e9908c8208090c9c3d3a882e56772065";
  best = english "best" "tessdata_best" "8280aed0782fe27257a68ea10fe7ef324ca0f8d85bd2fd145d1c2b560bcb66ba";
in
pkgs.runCommand "wros-hp-measure" {
  nativeBuildInputs = [
    python pkgs.typst pkgs.ghostscript poppler pkgs.makeWrapper
  ] ++ lib.optional (jbig2 != null) jbig2;
  meta.mainProgram = "wros-hp-measure";
} ''
  share="$out/share/hp-measure"
  models="$share/models"
  mkdir -p "$out/bin" "$models/fast" "$models/standard" "$models/best"
  ln -s ${fast} "$models/fast/eng.traineddata"
  ln -s ${standard} "$models/standard/eng.traineddata"
  ln -s ${best} "$models/best/eng.traineddata"
  for lang in chi_sim chi_sim_vert; do
    data=$(find -L ${tesseract} -name "$lang.traineddata" -type f -print -quit)
    test -n "$data"
    ln -s "$data" "$models/fast/$lang.traineddata"
  done

  export SOURCE_DATE_EPOCH=1
  python3 -B ${recipe}/make-corpus.py --output "$share/corpus" --bench ${bench} \
    --font-path ${pkgs.liberation_ttf}/share/fonts \
    --font-path ${pkgs.noto-fonts-cjk-sans}/share/fonts \
    ${lib.optionalString (jbig2 != null) "--jbig2 ${jbig2}/bin/jbig2"}

  test "$(pdfinfo "$share/corpus/book.pdf" | awk '/^Pages:/ { print $2 }')" = 300
  for name in chinese vertical; do
    pdffonts "$share/corpus/$name.pdf" | grep -F CID
  done

  makeWrapper ${pkgs.python3}/bin/python3 "$out/bin/wros-hp-measure" \
    --add-flags "-B ${bench} hp-measure \
      --corpus $share/corpus --pdfium ${pdfium}/lib/libpdfium.so \
      --pdftoppm ${poppler}/bin/pdftoppm --pdftotext ${poppler}/bin/pdftotext \
      --tesseract ${tesseract}/bin/tesseract --hayro ${hayroBench}/bin/wros-hayro-bench \
      --model-dir fast=$models/fast --model-dir standard=$models/standard --model-dir best=$models/best"

  # The kit checks itself as it is built (manifest hashes, the wrapper's arguments, a render and
  # extract), rather than in python-suite, which would otherwise build all of this for every run.
  python3 -B ${recipe}/test-kit.py "$out"
''
