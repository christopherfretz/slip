#include "side.h"
// §4.4 "Spellings", rule 1 and rule 11: `spelling/full`: a kind walked in full by profile mode (`__builtin_choose_expr`) holding a `decltype` prefix of an unresolved name (`DependentScopeDeclRefExpr`) whose operand holds the found set of `f`; the full walk, off unit level, raises the mark at the qualifier.
template<int N> struct Bq { static constexpr int value = 1; };
int f(int);
template<class T> auto g(T t) -> decltype(__builtin_choose_expr(1, (T *)0, (T *)decltype(Bq<sizeof(sizeof(f(T())))>{})::value));
