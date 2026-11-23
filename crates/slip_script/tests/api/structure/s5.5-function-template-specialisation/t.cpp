template <class T>
void tf(T) {}
void use() {
  tf(1.0);
  tf<int>(1);
}
