#include "side.h"
// §10.1 LoopHintState: `AssumeSafety`: the fact differs.
#ifdef B
void f() {
#pragma clang loop vectorize(assume_safety)
  for (;;) { }
}
#else
void f() {
#pragma clang loop vectorize(enable)
  for (;;) { }
}
#endif
