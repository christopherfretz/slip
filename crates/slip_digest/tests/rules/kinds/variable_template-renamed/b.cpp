template <class T> constexpr int u = sizeof(T);
int use() { return u<int>; }
