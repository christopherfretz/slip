#include "side.h"
// §7 _ `concept-result-checked-elsewhere`: the stored result `ConceptSpecializationExpr.aux` is the expression's own: the same concept checked with the same arguments elsewhere, before and after.
template<class T> concept C = sizeof(T) == 4;
template<class T> concept D = true;
#ifdef B
bool j = C<int>;
template<class T> bool cv = C<T>;
bool i = cv<int>;
bool k = C<int>;
#else
template<class T> bool cv = C<T>;
bool i = cv<int>;
#endif
