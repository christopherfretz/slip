#include "side.h"
// §5.5 RecordData `bases`: two bases swapped (digest §5: base order is kept).
#ifdef B
struct P {}; struct Q {}; struct C : Q, P {};
#else
struct P {}; struct Q {}; struct C : P, Q {};
#endif
