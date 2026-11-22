#include "side.h"
// §10.1 LoopHintState: `Disable`: the fact differs.
#ifdef B
void f() {
#pragma clang loop unroll(disable)
  for (;;) { }
}
#else
void f() {
#pragma clang loop unroll(enable)
  for (;;) { }
}
#endif
