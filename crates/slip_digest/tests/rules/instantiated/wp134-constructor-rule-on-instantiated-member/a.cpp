template <class T> struct W { T v; W(T x); };
int f() { W<int> w(1); return w.v; }
