#include "side.h"
// §10.1 LoopHintOption: `Distribute`: the fact differs.
#ifdef B
void f() {
#pragma clang loop distribute(enable)
  for (;;) { }
}
#else
void f() {
#pragma clang loop vectorize(enable)
  for (;;) { }
}
#endif
