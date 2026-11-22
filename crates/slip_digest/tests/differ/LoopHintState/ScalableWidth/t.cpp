#include "side.h"
// §10.1 LoopHintState: `ScalableWidth`: the fact differs.
#ifdef B
void f() {
#pragma clang loop vectorize_width(4, scalable)
  for (;;) { }
}
#else
void f() {
#pragma clang loop vectorize_width(4, fixed)
  for (;;) { }
}
#endif
