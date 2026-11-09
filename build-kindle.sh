#!/bin/sh
# Script para compilar Kauwlendar para Kindle (ARM musl estático)
set -e

cd "$(dirname "$0")"

echo "Compilando Kauwlendar para Kindle (armv7-unknown-linux-musleabi)..."

CC_armv7_unknown_linux_musleabi=clang \
CFLAGS_armv7_unknown_linux_musleabi="-target armv7-unknown-linux-musleabi" \
AR_armv7_unknown_linux_musleabi=llvm-ar \
cargo build --release --target armv7-unknown-linux-musleabi

echo "¡Compilación exitosa!"
echo "Binario generado en: target/armv7-unknown-linux-musleabi/release/kauwlendar"
ls -lh target/armv7-unknown-linux-musleabi/release/kauwlendar

