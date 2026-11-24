template <class T> struct W { T v; bool operator==(const W &o) const { return v == o.v; } };
int f(W<int> a, W<int> b) { return a == b; }
