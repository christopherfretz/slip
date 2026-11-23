#include "side.h"
// §7 _ `concept-result-checked-elsewhere`: the stored result `ConceptSpecializationExpr.aux` is the expression's own: the same concept checked with the same arguments elsewhere, before and after (in functions: a variable added would move the order unit, digest §2.1).
template<class T> concept C = sizeof(T) == 4;
template<class T> concept D = true;
#ifdef B
void j() { (void)C<int>; }
template<class T> bool cv = C<T>;
bool i = cv<int>;
void k() { (void)C<int>; }
#else
template<class T> bool cv = C<T>;
bool i = cv<int>;
#endif
