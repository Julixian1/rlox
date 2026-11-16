#!/bin/bash
# Uso:
#   ./tests.sh          corre los tres backends (tree-walk, VM de pila y VM de registros)
#   ./tests.sh --tree   solo el tree-walk
#   ./tests.sh --vm     solo la VM de bytecode (pila)
#   ./tests.sh --reg    solo la VM de registros
#   ./tests.sh -v       además imprime la salida completa de cada prueba
# Los flags de backend se pueden combinar: ./tests.sh --vm --reg

RUN_TREE=0; RUN_VM=0; RUN_REG=0
VERBOSE=0
for arg in "$@"; do
    case "$arg" in
        --tree) RUN_TREE=1 ;;
        --vm) RUN_VM=1 ;;
        --reg) RUN_REG=1 ;;
        -v) VERBOSE=1 ;;
    esac
done
# Sin ningún flag de backend: corre todos.
if [ $((RUN_TREE + RUN_VM + RUN_REG)) -eq 0 ]; then
    RUN_TREE=1; RUN_VM=1; RUN_REG=1
fi

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
[ "$RUN_TREE" -eq 1 ] && { run_suite "TREE-WALK" "" || status=1; }
[ "$RUN_VM" -eq 1 ]   && { run_suite "VM (bytecode, pila)" "--vm" || status=1; }
[ "$RUN_REG" -eq 1 ]  && { run_suite "VM (bytecode, registros)" "--reg" || status=1; }

echo
if [ "$status" -eq 0 ]; then
    echo "Todas las pruebas pasaron."
else
    echo "Hay pruebas que fallan."
fi
exit $status
