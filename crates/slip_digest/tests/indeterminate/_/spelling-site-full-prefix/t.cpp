#include "side.h"
// §4.4 "Spellings", rule 1 and rule 10: `spelling/full`: a kind walked in full by profile mode (`__builtin_choose_expr`) holding a resolved name whose qualifier holds the found set of `f`; the full walk, off unit level, raises the mark at the name.
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
template<class T> auto g(T t) -> decltype(__builtin_choose_expr(1, (T *)0, (T *)Bq<sizeof(sizeof(f(T())))>::value));
