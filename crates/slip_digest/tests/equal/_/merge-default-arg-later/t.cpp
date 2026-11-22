#include "side.h"
// §2.3, §5.4 ParamData `default_arg`: from the declaration that gives it.
#ifdef B
void f(int); void f(int = 1);
#else
void f(int = 1); void f(int);
#endif
