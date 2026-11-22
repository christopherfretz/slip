#include "side.h"
// §5.4 FunctionData `params`: a parameter's name is not in its stream.
#ifdef B
void f(int b);
#else
void f(int a);
#endif
