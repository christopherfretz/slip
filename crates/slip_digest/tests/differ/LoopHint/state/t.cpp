#include "side.h"
// §10.1 LoopHint: `state`: the fact differs.
#ifdef B
void f() {
#pragma clang loop unroll(full)
  for (;;) { }
}
#else
void f() {
#pragma clang loop unroll(enable)
  for (;;) { }
}
#endif
