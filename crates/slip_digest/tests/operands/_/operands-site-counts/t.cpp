#include "side.h"
// WP-156 fix round 1 (S4: mutations R09, R10): each holder's entry is
// written once. A function's parameters are read by the groups of its
// declarations (`operands::begin`), not again where the stream meets
// them (`operands::decl`): a pattern's parameters (its pattern is
// covered, R09), and a parameter whose function is covered (R10). The
// sides are the same: the counters show an entry written twice, which
// no set of names and no pair does.
namespace lib { struct X {}; int make(X); }
lib::X x;
template <class T> struct Wr { T m; };
template <class T> void ft(T, decltype(make(x)) q);
template <class T> void fd(T, Wr<decltype(make(x))> q) {}
void h(lib::X p, decltype(make(x)) q);
void hd(lib::X p, decltype(make(x)) q) {}
