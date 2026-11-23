template <class T> concept D = sizeof(T) > 1;
template <D T> int f(T) { return 1; }
int use() { return f(1); }
