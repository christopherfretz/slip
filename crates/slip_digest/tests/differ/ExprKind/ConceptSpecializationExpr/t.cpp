#include "side.h"
// §7 ExprKind `ConceptSpecializationExpr`.
template<class T> concept C = sizeof(T) == 4;
template<class T> concept D = true;
#ifdef B
template<class T> bool cv = true;
#else
template<class T> bool cv = C<T>;
#endif
