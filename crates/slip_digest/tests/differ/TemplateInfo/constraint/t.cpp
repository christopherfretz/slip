#include "side.h"
// §9.2 TemplateInfo `constraint`: a concept's constraint-expression.
#ifdef B
template <class T> concept C = sizeof(T) == 8;
#else
template <class T> concept C = sizeof(T) == 4;
#endif
