#include "side.h"
// §10.1 LoopHintOption: `PipelineInitiationInterval`: the fact differs.
#ifdef B
void f() {
#pragma clang loop pipeline_initiation_interval(4)
  for (;;) { }
}
#else
void f() {
#pragma clang loop unroll_count(4)
  for (;;) { }
}
#endif
