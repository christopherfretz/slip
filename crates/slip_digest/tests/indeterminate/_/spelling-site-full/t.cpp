#include "side.h"
// §4.4 "Spellings", rule 1: `spelling/full`, a kind walked in full by profile mode (`__builtin_choose_expr`) whose own walk left a fact out (the found set of `f` in a type inside it). A lambda cannot reach it in a signature: its closure type is a local entity (UndeclaredLocal).
int f(int);
template<class T> auto g(T t) -> decltype(__builtin_choose_expr(1, (decltype(f(T())) *)0, 0));
