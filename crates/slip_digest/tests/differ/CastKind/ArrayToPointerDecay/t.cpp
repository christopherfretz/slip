#include "side.h"
// §7.2 CastKind `ArrayToPointerDecay`.
int arr[2]; int *p;
#ifdef B
int *v = p;
#else
int *v = arr;
#endif
