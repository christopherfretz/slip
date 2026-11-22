#include "side.h"
// §4.3 TypeView `LValueRef.0`.
#ifdef B
extern char& v;
#else
extern int& v;
#endif
