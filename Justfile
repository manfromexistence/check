set shell := ["pwsh.exe", "-c"]

build:
    cargo build --release -j 12
    @New-Item -ItemType Directory -Force -Path G:\Dx\bin | Out-Null
    @Copy-Item target\release\dx-check.exe G:\Dx\bin\dx-check.exe -Force





