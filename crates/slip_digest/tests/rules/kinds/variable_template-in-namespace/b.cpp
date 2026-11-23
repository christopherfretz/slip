namespace legacy {
template <class T> constexpr int v = sizeof(T);
template <> constexpr int v<char> = 7;
}
int use() { return legacy::v<int> + legacy::v<char>; }
