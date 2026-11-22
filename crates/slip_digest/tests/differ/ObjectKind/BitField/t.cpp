#include "side.h"
// §7.1 ObjectKind `BitField`: the same member, a bit-field on one side
// only, so `v` differs by its object kind alone (keys are not content).
struct BF {
#ifdef B
  int b;
#else
  int b : 3;
#endif
};
BF bf;
int v = bf.b;
