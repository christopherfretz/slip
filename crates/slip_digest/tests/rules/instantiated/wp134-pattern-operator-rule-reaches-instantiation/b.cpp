template <class T> struct W { T v; bool operator==(const W &o) const { return v == o.v; } W(T x); };
int f(W<int> a, W<int> b) { W<int> c(1); return a.operator==(b) + c.v; }
