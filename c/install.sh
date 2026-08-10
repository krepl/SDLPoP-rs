#! /bin/bash

cd ..
# Since we don't copy the executable or the data folder anywhere, the desktop file has to be updated to contain the actual paths.
sed -e 's|$ROOT|'"$PWD"'|' c/SDLPoP.desktop.template > c/SDLPoP.desktop
cp c/SDLPoP.desktop /usr/share/applications/

