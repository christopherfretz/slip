#include "side.h"
// §5.3 DeclNode `FunctionTemplate.data`: a default template argument.
#ifdef B
template <class T = int> void g(T);
#else
template <class T> void g(T);
#endif
