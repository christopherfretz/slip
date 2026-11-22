#include "side.h"
// §10.1 LoopHintState: `FixedWidth`: the fact differs.
#ifdef B
void f() {
#pragma clang loop vectorize_width(4, fixed)
  for (;;) { }
}
#else
void f() {
#pragma clang loop vectorize_width(4, scalable)
  for (;;) { }
}
#endif
