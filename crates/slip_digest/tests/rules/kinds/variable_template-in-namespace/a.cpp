namespace acme {
template <class T> constexpr int v = sizeof(T);
template <> constexpr int v<char> = 7;
}
int use() { return acme::v<int> + acme::v<char>; }
