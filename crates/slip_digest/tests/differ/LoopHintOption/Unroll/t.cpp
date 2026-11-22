#include "side.h"
// §10.1 LoopHintOption: `Unroll`: the fact differs.
#ifdef B
void f() {
#pragma clang loop unroll(enable)
  for (;;) { }
}
#else
void f() {
#pragma clang loop vectorize(enable)
  for (;;) { }
}
#endif
