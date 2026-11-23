int zero_first() {
  int a[2] = {1, 2};
  for (auto x : a)
    x = 0;
  return a[0];
}
