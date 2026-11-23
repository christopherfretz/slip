template <class T> struct A { template <class U> friend struct V; };
template <class U> struct V { int x = 1; };
A<int> a;
int u() { return V<int>().x; }
