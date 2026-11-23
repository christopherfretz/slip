template <class T> struct V { V(T); };
V(const char *) -> V<int>;
auto w = V(1);
