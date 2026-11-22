#include "side.h"
// §10.1 LoopHintState: `Full`: the fact differs.
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
