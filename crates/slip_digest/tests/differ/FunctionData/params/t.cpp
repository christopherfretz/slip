#include "side.h"
// §5.4 FunctionData `params`: a top-level const on a parameter is not in the key but is in the parameter's stream.
#ifdef B
void f(const int a);
#else
void f(int a);
#endif
