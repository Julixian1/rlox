#!/bin/bash
echo "Compilando proyecto..."
cargo build --quiet || exit 1

echo -e "\n=== EJECUTANDO TESTS ==="
for file in real-tests/*.lox; do
    echo -e "\n----------------------------------------"
    echo "Ejecutando: $file"
    echo "----------------------------------------"
    ./target/debug/rlox "$file"
done