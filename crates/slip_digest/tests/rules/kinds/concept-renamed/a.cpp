template <class T> concept C = sizeof(T) > 1;
template <C T> int f(T) { return 1; }
int use() { return f(1); }
