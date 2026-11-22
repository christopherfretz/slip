#include "side.h"
// §5.4 ParamData `default_arg`: a default argument against none.
#ifdef B
void f(int a = 1);
#else
void f(int a);
#endif
