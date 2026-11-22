#include "side.h"
// §10.1 LoopHintSpelling: `Unroll`: the fact differs.
#ifdef B
void f() {
#pragma unroll
  for (;;) { }
}
#else
void f() {
#pragma clang loop unroll(full)
  for (;;) { }
}
#endif
