namespace legacy { template <class T> concept C = sizeof(T) > 1; }
template <legacy::C T> int f(T) { return 1; }
template <class T> int f(T) { return 2; }
template <class T> requires legacy::C<T> int h(T) { return 3; }
int use() { return f(1) + f('c') + h(1); }
