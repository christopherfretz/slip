#include "side.h"
// spec/digest-kinds.md §3, §5.2: the key of a concept.
#ifdef B
template <class T> concept D = true;
#else
template <class T> concept C = true;
#endif
