#include "side.h"
// §5.4 ParamData `original_ty`: an array parameter against a pointer one, the same adjusted type.
#ifdef B
void f(int a[3]);
#else
void f(int* a);
#endif
