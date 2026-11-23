template <class T> struct W { W(const char *) { int a = 0; } };
template <class T> struct V { V(const char *) {} };
W<int> w("x");
