template <class T> struct W { T v; int n() const { struct L { T x; operator T() const { return x; } }; return int(T(L{v})); } template <class U> int nt(U u) const { struct M { U y; operator U() const { return y; } }; return int(U(M{u})); } };
int f(W<int> w) { return w.n() + w.nt(2L); }
