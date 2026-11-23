template <class T> constexpr int v = sizeof(T);
int use() { return v<int>; }
