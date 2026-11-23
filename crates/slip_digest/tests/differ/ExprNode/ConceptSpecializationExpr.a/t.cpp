#include "side.h"
// §7 ExprNode `ConceptSpecializationExpr.a`: the concept.
template<class T> concept C = sizeof(T) == 4;
template<class T> concept D = true;
#ifdef B
template<class T> bool cv = D<T>;
#else
template<class T> bool cv = C<T>;
#endif
