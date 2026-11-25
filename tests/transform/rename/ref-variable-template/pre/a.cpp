namespace a {
template <class T> int v = 1;
template <class T> int v<T *> = 2;
}
int u() { return a::v<int> + a::v<int *>; }
