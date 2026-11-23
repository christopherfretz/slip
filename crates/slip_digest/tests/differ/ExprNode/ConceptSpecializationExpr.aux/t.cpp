#include "side.h"
// §7 ExprNode `ConceptSpecializationExpr.aux`: the stored result of an instantiation.
template<class T> concept C = sizeof(T) == 4;
template<class T> concept D = true;
template<class T> bool cv = C<T>;
#ifdef B
struct X { long long a; };
bool i = cv<X>;
#else
struct X { int a; };
bool i = cv<X>;
#endif
