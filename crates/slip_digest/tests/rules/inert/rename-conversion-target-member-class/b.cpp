template <class T> struct W { T v; struct Inner { int q; }; operator Inner() const { return {1}; } operator Inner *() const { return nullptr; } };
int f(W<bool> s) { W<bool>::Inner i = s; W<bool>::Inner *p = s; return i.q + (p == nullptr); }
