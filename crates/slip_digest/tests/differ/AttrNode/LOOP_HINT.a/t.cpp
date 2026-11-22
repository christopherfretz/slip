#include "side.h"
// §10.1 AttrNode: `LOOP_HINT.a`: the fact differs.
#ifdef B
void f() {
#pragma clang loop unroll_count(4)
  for (;;) { }
}
#else
void f() {
#pragma clang loop unroll_count(2)
  for (;;) { }
}
#endif
