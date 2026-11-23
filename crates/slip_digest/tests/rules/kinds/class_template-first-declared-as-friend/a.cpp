template <class T> struct A { template <class U> friend struct W; };
template <class U> struct W { int x = 1; };
A<int> a;
int u() { return W<int>().x; }
