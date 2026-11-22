#include "side.h"
// §5.4 ParamData `default_arg`.
#ifdef B
void f(int a = 1);
#else
void f(int a = 2);
#endif
