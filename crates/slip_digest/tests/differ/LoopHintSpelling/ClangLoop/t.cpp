#include "side.h"
// §10.1 LoopHintSpelling: `ClangLoop`: the fact differs.
#ifdef B
void f() {
#pragma clang loop unroll(full)
  for (;;) { }
}
#else
void f() {
#pragma unroll
  for (;;) { }
}
#endif
