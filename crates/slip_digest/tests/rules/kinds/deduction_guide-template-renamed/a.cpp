template <class T> struct W { W(T); };
W(const char *) -> W<int>;
auto w = W(1);
