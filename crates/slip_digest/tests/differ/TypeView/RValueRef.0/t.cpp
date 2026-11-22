#include "side.h"
// §4.3 TypeView `RValueRef.0`.
#ifdef B
extern long&& v;
#else
extern int&& v;
#endif
