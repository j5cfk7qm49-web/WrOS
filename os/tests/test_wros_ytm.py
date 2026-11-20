"""The public YouTube Music helper with a stand-in client and no network."""
from contextlib import redirect_stderr, redirect_stdout
import importlib.machinery
import importlib.util
import io
import json
from pathlib import Path
import sys
from types import ModuleType
import unittest
from unittest.mock import Mock, call, patch


SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/wros-ytm'
LOADER = importlib.machinery.SourceFileLoader('wros_ytm', str(SCRIPT))
HELPER = importlib.util.module_from_spec(importlib.util.spec_from_loader(LOADER.name, LOADER))
LOADER.exec_module(HELPER)


class YouTubeMusic(unittest.TestCase):
    def setUp(self):
        self.client = Mock()
        fake = ModuleType('ytmusicapi')
        fake.YTMusic = self.factory = Mock(return_value=self.client)
        modules = patch.dict(sys.modules, {'ytmusicapi': fake})
        modules.start()
        self.addCleanup(modules.stop)

    def run_helper(self, *args):
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = HELPER.main(args)
        self.assertEqual(len(stdout.getvalue().splitlines()), 1)
        return status, json.loads(stdout.getvalue())

    def test_search_shape_and_limit(self):
        song = {'videoId': 'Song_1', 'title': 'Song', 'duration': '2:35',
                'artists': [{'name': 'One'}, {'name': 'Two'}]}
        album = {'browseId': 'MPREbAlbum', 'title': 'Album', 'artist': 'One'}
        playlist = {'browseId': 'VLPLList', 'title': 'List', 'author': 'Three',
                    'itemCount': '1,200'}
        self.client.search.side_effect = [[song] * 21, [album], [playlist]]

        status, result = self.run_helper('search', 'song')
        self.factory.assert_called_once_with()
        self.assertEqual(self.client.search.call_args_list, [
            call('song', filter=kind, limit=20) for kind in ('songs', 'albums', 'playlists')
        ])
        self.assertEqual(status, 0)
        self.assertEqual(result, {
            'songs': [{'id': 'Song_1', 'title': 'Song', 'artist': 'One, Two',
                       'length': '2:35', 'count': None}] * 20,
            'albums': [{'id': 'MPREbAlbum', 'title': 'Album', 'artist': 'One',
                        'length': '', 'count': None}],
            'playlists': [{'id': 'VLPLList', 'title': 'List', 'artist': 'Three',
                           'length': '', 'count': 1200}],
        })

    def test_album_and_playlist_tracks(self):
        track = {'videoId': 'Song-1', 'title': 'Song', 'duration': '3:12'}
        self.client.get_album.return_value = {
            'title': 'Album', 'artists': [{'name': 'Artist'}],
            'tracks': [track, {'title': 'Unavailable', 'videoId': None}],
        }
        self.client.get_playlist.return_value = {'title': 'Playlist', 'tracks': [track]}
        item = {'id': 'Song-1', 'title': 'Song', 'artist': 'Artist', 'length': '3:12'}

        self.assertEqual(self.run_helper('tracks', 'MPREbAlbum'),
                         (0, {'title': 'Album', 'tracks': [item]}))
        self.client.get_album.assert_called_once_with('MPREbAlbum')
        self.client.get_playlist.assert_not_called()
        for browse_id in ('VLPLList', 'PLList'):
            self.assertEqual(self.run_helper('tracks', browse_id),
                             (0, {'title': 'Playlist', 'tracks': [dict(item, artist='')]}))
        self.assertEqual(self.client.get_playlist.call_args_list, [call('PLList', limit=300)] * 2)

    def test_bad_ids_and_queries_are_rejected_before_using_the_client(self):
        for value in ('', 'x', 'a' * 81, 'a/b', 'aa\n', 'éé'):
            with self.subTest(value=value):
                self.assertEqual(self.run_helper('tracks', value),
                                 (1, {'error': 'invalid YouTube Music id'}))
        for query in ('a' * 201, 'a\tb'):
            self.assertEqual(self.run_helper('search', query)[0], 1)
        self.factory.assert_not_called()

    def test_client_error_is_single_short_json_object(self):
        def fail(*args, **kwargs):
            print('noisy client')
            raise RuntimeError('network unavailable\nlong details')

        self.client.search.side_effect = fail
        self.assertEqual(self.run_helper('search', 'song'), (1, {'error': 'network unavailable'}))
