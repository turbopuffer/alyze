import gzip
import hashlib
import io
from pathlib import Path
import sys
import tarfile

root = Path(__file__).resolve().parents[1]
output = Path(sys.argv[1]) if len(sys.argv) > 1 else root / 'target/dictionaries'
output.mkdir(parents=True, exist_ok=True)

for language in ('smartcn', 'nori', 'kuromoji'):
    files = sorted((root / 'data').glob('*.bin') if language == 'smartcn' else (root / 'data' / language).iterdir())
    files += [root / 'LICENSE-APACHE', root / 'data/NOTICE', root / 'LICENSE-mecab-ipadic']
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode='w', format=tarfile.USTAR_FORMAT) as tar:
        for path in files:
            data = path.read_bytes()
            name = path.relative_to(root / 'data').as_posix() if path.is_relative_to(root / 'data') else path.name
            entry = tarfile.TarInfo("LICENSE" if name == "LICENSE-APACHE" else name)
            entry.size = len(data)
            entry.mode = 0o644
            tar.addfile(entry, io.BytesIO(data))
    compressed = io.BytesIO()
    with gzip.GzipFile(fileobj=compressed, mode="wb", filename="", compresslevel=9, mtime=0) as stream:
        stream.write(archive.getvalue())
    data = compressed.getvalue()
    path = output / f'{language}.tar.gz'
    path.write_bytes(data)
    print(f'{hashlib.sha256(data).hexdigest()}  {path.name}  ({len(data)} bytes)')
