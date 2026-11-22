#include "side.h"
// §10.1 LoopHintState: `Numeric`: the fact differs.
#ifdef B
void f() {
#pragma clang loop vectorize_width(4)
  for (;;) { }
}
#else
void f() {
#pragma clang loop vectorize_width(scalable)
  for (;;) { }
}
#endif
