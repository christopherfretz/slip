#include "side.h"
namespace lib { struct X {};
#ifndef B
int make(X);
#endif
}
template <class T> int make(T);
lib::X x;
template <int N> int vt = N;
int o_vt = vt<sizeof(make(x))>;
template <int N> int ft() { return N; }
int o_ft = ft<sizeof(make(x))>();
template <int N> concept C = N > 0;
bool o_c = C<sizeof(make(x))>;
template <class T> concept CT = true;
bool o_ct = CT<decltype(make(x))>;
template <int N> struct o_es {};
template <> struct o_es<sizeof(make(x))> {};
