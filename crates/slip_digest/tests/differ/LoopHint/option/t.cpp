#include "side.h"
// §10.1 LoopHint: `option`: the fact differs.
#ifdef B
void f() {
#pragma clang loop interleave(enable)
  for (;;) { }
}
#else
void f() {
#pragma clang loop vectorize(enable)
  for (;;) { }
}
#endif
