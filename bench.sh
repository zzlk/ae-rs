sudo nice -n-10 cargo bench -- --measurement-time 15 -n
echo "Benchmark Commit" >/tmp/txt
echo "" >>/tmp/txt
critcmp base.json new >> /tmp/txt
critcmp --export base > base.json
git add base.json
git commit --allow-empty -F /tmp/txt
