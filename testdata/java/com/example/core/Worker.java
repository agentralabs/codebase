package com.example.core;

import java.util.ArrayList;
import java.util.List;
import java.util.function.Consumer;
import com.example.shared.Helper;

public class Worker extends Base implements Workable {
    private List<String> names = new ArrayList<>();
    private Helper helper = new Helper();

    public Worker() {
        super();
    }

    @Override
    public void process(String item) {
        Helper.log(item);
        names.add(item);

        Runnable r = new Runnable() {
            @Override
            public void run() {
                Helper.log("anon");
            }
        };

        Consumer<String> consumer = s -> Helper.log(s);
        r.run();
        consumer.accept(item);
    }

    public void process(String item, int count) {
        for (int i = 0; i < count; i++) {
            process(item);
        }
    }
}

