#include "side.h"
// §4.3 TypeView `LValueRef`.
#ifdef B
extern int&& v;
#else
extern int& v;
#endif
