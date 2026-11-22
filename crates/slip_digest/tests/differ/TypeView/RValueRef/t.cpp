#include "side.h"
// §4.3 TypeView `RValueRef`.
#ifdef B
extern int* v;
#else
extern int&& v;
#endif
