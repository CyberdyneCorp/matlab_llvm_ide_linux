% sim3d_demo.m — minimal sim3d script for the IDE e2e test.
% Running it writes sim3d_demo.html (a Babylon 3-D scene) which the IDE
% auto-opens in the embedded 3-D Scene viewer.
w = sim3d.World();

ground = sim3d.Actor('ground', 'plane');
ground.Size = [20 20 1];
ground.Color = [0.18 0.19 0.22];
w.add(ground);

car = sim3d.Actor('vehicle', 'box');
car.Size = [2.0 1.0 0.6];
car.Color = [0.85 0.2 0.2];
w.add(car);

w.open();
x = 0;
for k = 1:30
    x = x + 0.3;
    car.Translation = [x 0 0.3];
    w.run(0.02);
end
w.close();

sim3d.export(w, 'sim3d_demo.html');
disp('wrote sim3d_demo.html');
