#include "side.h"
// digest §4.3: `struct Q *p` declares the local class Q in f's block
// ([basic.scope.pdecl]/7) with no declaration statement of its own.
int f() { struct Q *p = nullptr; return p ? 1 : 0; }
