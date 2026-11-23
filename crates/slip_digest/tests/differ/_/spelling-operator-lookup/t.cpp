#include "side.h"
// §4.4 "Spellings": the non-member operators a dependent operator found at the definition (oper_lookups), left out of the profile like a found set.
struct X {};
X operator+(X, X);
template<class T> auto g(T t) -> decltype(t + t);
#ifdef B
struct Y {};
Y operator+(Y, Y);
#endif
template<class T> auto h(T t) -> decltype(t + t);
