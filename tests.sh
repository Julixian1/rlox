#!/bin/bash
# Uso:
#   ./tests.sh          corre los dos backends (tree-walk y VM)
#   ./tests.sh --vm     solo la VM de bytecode
#   ./tests.sh --tree   solo el tree-walk
#   ./tests.sh -v       además imprime la salida completa de cada prueba
 
MODE="both"
VERBOSE=0
for arg in "$@"; do
    case "$arg" in
        --vm) MODE="vm" ;;
        --tree) MODE="tree" ;;
        -v) VERBOSE=1 ;;
    esac
done
 
echo "Compilando proyecto..."
cargo build --quiet || exit 1
 
run_suite() {
    local name="$1"; shift
    local flag="$1"
    local failed=0
 
    echo -e "\n=== $name ==="
    for file in real-tests/*.lox; do
        output=$(./target/debug/rlox $flag "$file" 2>&1)
        ok=$(echo "$output" | grep -c "OK")
        errors=$(echo "$output" | grep -c "ERROR")
 
        if [ "$errors" -eq 0 ] && [ "$ok" -gt 0 ]; then
            echo "  PASA   $file ($ok OK)"
        else
            echo "  FALLA  $file ($ok OK, $errors ERROR)"
            failed=1
        fi
 
        if [ "$VERBOSE" -eq 1 ]; then
            echo "$output" | sed 's/^/      /'
        fi
    done
    return $failed
}
 
status=0
if [ "$MODE" != "vm" ]; then
    run_suite "TREE-WALK" "" || status=1
fi
if [ "$MODE" != "tree" ]; then
    run_suite "VM (bytecode)" "--vm" || status=1
fi
 
echo
if [ "$status" -eq 0 ]; then
    echo "Todas las pruebas pasaron."
else
    echo "Hay pruebas que fallan."
fi
exit $status