#include "side.h"
// §10.1 LoopHintState: `Enable`: the fact differs.
#ifdef B
void f() {
#pragma clang loop unroll(enable)
  for (;;) { }
}
#else
void f() {
#pragma clang loop unroll(disable)
  for (;;) { }
}
#endif
