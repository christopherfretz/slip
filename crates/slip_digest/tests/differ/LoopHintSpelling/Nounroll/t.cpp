#include "side.h"
// §10.1 LoopHintSpelling: `Nounroll`: the fact differs.
#ifdef B
void f() {
#pragma nounroll
  for (;;) { }
}
#else
void f() {
#pragma clang loop unroll(disable)
  for (;;) { }
}
#endif
