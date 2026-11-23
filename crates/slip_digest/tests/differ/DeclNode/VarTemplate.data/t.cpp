// std: c++17
#include "side.h"
// §5.3 DeclNode `VarTemplate.data`: a default template argument.
#ifdef B
template <class T = int> T w;
#else
template <class T> T w;
#endif
