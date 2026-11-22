#include "side.h"
// §5.4 ParamData `flags-scope-depth`: a parameter of a parameter's function type is one scope deeper.
#ifdef B
void f(void (*g)(int b));
#else
void f(void (*g)(int));
#endif
