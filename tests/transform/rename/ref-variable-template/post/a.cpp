namespace a {
template <class T> int w = 1;
template <class T> int w<T *> = 2;
}
int u() { return a::w<int> + a::w<int *>; }
