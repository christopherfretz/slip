// std: c++20
#include "side.h"
// §5.3 DeclNode `Concept.data`: the template head (in the key, §3.3).
#ifdef B
template <class T, class U> concept C = true;
#else
template <class T> concept C = true;
#endif
