template <class T> struct W { T v; struct In { int q; }; operator In() const { return {1}; } operator In *() const { return nullptr; } };
int f(W<bool> s) { W<bool>::In i = s; W<bool>::In *p = s; return i.q + (p == nullptr); }
